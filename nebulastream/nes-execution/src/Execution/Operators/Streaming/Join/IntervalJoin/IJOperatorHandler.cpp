/*
    Licensed under the Apache License, Version 2.0 (the "License");
    you may not use this file except in compliance with the License.
    You may obtain a copy of the License at

        https://www.apache.org/licenses/LICENSE-2.0

    Unless required by applicable law or agreed to in writing, software
    distributed under the License is distributed on an "AS IS" BASIS,
    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
    See the License for the specific language governing permissions and
    limitations under the License.
*/

#include <API/AttributeField.hpp>
#include <Execution/Operators/Streaming/Join/IntervalJoin/IJOperatorHandler.hpp>
#include <Execution/Operators/Streaming/MultiOriginWatermarkProcessor.hpp>
#include <Execution/Operators/Streaming/TimeFunction.hpp>
#include <Nautilus/Interface/DataTypes/Value.hpp>
#include <Nautilus/Interface/PagedVector/PagedVectorVarSizedRef.hpp>
#include <Runtime/Execution/PipelineExecutionContext.hpp>
#include <Util/magicenum/magic_enum.hpp>
#include <fstream>
#include <map>

namespace NES::Runtime::Execution::Operators {

IJOperatorHandler::IJOperatorHandler(const std::vector<OriginId>& inputOrigins,
                                     const OriginId outputOriginId,
                                     const int64_t lowerBound,
                                     const int64_t upperBound,
                                     const SchemaPtr& leftSchema,
                                     const SchemaPtr& rightSchema,
                                     const uint64_t leftPageSize,
                                     const uint64_t rightPageSize)
    : pageSizeRight(rightPageSize), pageSizeLeft(leftPageSize), numberOfWorkerThreads(1), outputOriginId(outputOriginId),
      lowerBound(lowerBound), upperBound(upperBound), sizeOfRecordLeft(leftSchema->getSchemaSizeInBytes()),
      sizeOfRecordRight(rightSchema->getSchemaSizeInBytes()), leftSchema(leftSchema), rightSchema(rightSchema),
      watermarkProcessorBuild(std::make_unique<MultiOriginWatermarkProcessor>(inputOrigins)),
      watermarkProcessorProbe(std::make_unique<MultiOriginWatermarkProcessor>(std::vector<OriginId>(1, outputOriginId))),
      sequenceNumber(1) {}

void IJOperatorHandler::start(PipelineExecutionContextPtr pipelineCtx, uint32_t) {
    NES_INFO("Started IJOperatorHandler!");
    setNumberOfWorkerThreads(pipelineCtx->getNumberOfWorkerThreads());
    setBufferManager(pipelineCtx->getBufferManager());
    this->rightTuples->emplace_back(
        std::make_unique<Nautilus::Interface::PagedVectorVarSized>(bufferManager, this->rightSchema, this->pageSizeRight));
}

void IJOperatorHandler::setNumberOfWorkerThreads(uint64_t numberOfWorkerThreads) {
    if (IJOperatorHandler::alreadySetup) {
        NES_DEBUG("IJOperatorHandler::setup was called already!");
        return;
    }
    IJOperatorHandler::alreadySetup = true;
    NES_DEBUG("IJOperatorHandler::setup was called!");
    IJOperatorHandler::numberOfWorkerThreads = numberOfWorkerThreads;
}

void IJOperatorHandler::emitIntervalToProbe(auto& interval, PipelineExecutionContext* pipelineCtx) {

    if (interval->getNumberOfTuplesRight() >= 1) {
        // Gets new empty pooled tupleBuffer from BufferPoolManager
        auto tupleBuffer = pipelineCtx->getBufferManager()->getBufferBlocking();
        // Returns the buffer memory of the tuple buffer as an EmittedNLJWindowTriggerTask (containing information for a join window trigger)
        auto bufferMemory = tupleBuffer.getBuffer<EmittedIJTriggerTask>();
        // set information in buffer
        bufferMemory->intervalIdentifier = interval->getId();
        tupleBuffer.setNumberOfTuples(1);

        /** As we are here "emitting" a buffer, we have to set the originId, the seq number, and the watermark.
         *  The watermark can not be the interval end as some buffer might be still waiting for getting processed.
         */
        // operator id that creates this buffer
        tupleBuffer.setOriginId(getOutputOriginId());
        tupleBuffer.setSequenceData({getNextSequenceNumber(), /*chunkNumber*/ 1, true});
        tupleBuffer.setWatermark(static_cast<uint64_t>(interval->getIntervalStart()));

        interval->intervalState = IJIntervalInfoState::EMITTED_TO_PROBE;
        // The tupleBuffer is emitted to the Query manager, where a new task is created for it
        pipelineCtx->dispatchBuffer(tupleBuffer);

        NES_DEBUG("Emitted intervalId {} with watermarkTs {} sequenceNumber {} originId {} with no. right tuples {} and no. of "
                  "left tuples {}",
                  bufferMemory->intervalIdentifier,
                  tupleBuffer.getWatermark(),
                  tupleBuffer.getSequenceNumber(),
                  tupleBuffer.getOriginId(),
                  interval->getNumberOfTuplesRight(),
                  interval->getNumberOfTuplesLeft());
    } else {
        NES_INFO("Set tombstone for interval cause there were no right tuples present in interval {}", interval->toString());
        interval->intervalState = IJIntervalInfoState::MARKED_FOR_DELETION;
    }
}

std::optional<IJIntervalPtr> IJOperatorHandler::getIntervalByStartEnd(int64_t intervalStart, int64_t intervalEnd) {
    {
        auto currentIntervalsLocked = currentIntervals.rlock();
        for (auto& curInterval : *currentIntervalsLocked) {
            if (curInterval->getIntervalStart() == intervalStart && curInterval->getIntervalEnd() == intervalEnd) {
                return curInterval;
            }
        }
    }
    return std::nullopt;
}

uint64_t IJOperatorHandler::createAndAppendNewInterval(int64_t lowerIntervalBound, int64_t upperIntervalBound) {
    auto newInterval = std::make_shared<IJInterval>(lowerIntervalBound,
                                                    upperIntervalBound,
                                                    numberOfWorkerThreads,
                                                    bufferManager,
                                                    IJOperatorHandler::leftSchema,
                                                    IJOperatorHandler::pageSizeLeft,
                                                    IJOperatorHandler::rightSchema,
                                                    IJOperatorHandler::pageSizeRight);

    uint64_t intervalId;
    auto newIntervalStart = newInterval->getIntervalStart();
    auto newIntervalEnd = newInterval->getIntervalEnd();

    auto interval = getIntervalByStartEnd(newIntervalStart, newIntervalEnd);

    if (interval) {
        intervalId = interval->get()->getId();
    } else {
        {
            auto currentIntervalsLocked = currentIntervals.wlock();
            currentIntervalsLocked->push_back(newInterval);
            currentIntervalsLocked.unlock();
            intervalId = newInterval->getId();
        }
    }
    return intervalId;
}

void IJOperatorHandler::updateWatermarkForWorker(uint64_t watermark, WorkerThreadId workerThreadId) {
    workerThreadIdToWatermarkMap[workerThreadId] = watermark;
}

uint64_t IJOperatorHandler::getMinWatermarkForWorker() {
    auto minVal = std::min_element(workerThreadIdToWatermarkMap.begin(),
                                   workerThreadIdToWatermarkMap.end(),
                                   [](const auto& l, const auto& r) {
                                       return l.second < r.second;
                                   });
    return minVal == workerThreadIdToWatermarkMap.end() ? -1 : minVal->second;
}

uint64_t IJOperatorHandler::getNumberOfCurrentIntervals() { return this->currentIntervals->size(); }

std::string IJOperatorHandler::toString() {
    std::ostringstream basicOstringstream;
    basicOstringstream << "(IJHandlerId: " << getOutputOriginId() << " Lower Bound: " << lowerBound
                       << " Current Upper Bound: " << upperBound << " currentIntervals: " << currentIntervals->size() << ")";
    return basicOstringstream.str();
}

void* IJOperatorHandler::getPagedVectorRefRight(WorkerThreadId workerThreadId) {
    const auto pos = workerThreadId % rightTuples->size();
    return rightTuples->at(pos).get();
}

void IJOperatorHandler::checkAndTriggerIntervals(const BufferMetaData& bufferMetaData, PipelineExecutionContext* pipelineCtx) {
    // The watermark processor handles the minimal watermark across both streams
    uint64_t oldGlobalWatermark = watermarkProcessorBuild->getCurrentWatermark();
    uint64_t newGlobalWatermark =
        watermarkProcessorBuild->updateWatermark(bufferMetaData.watermarkTs, bufferMetaData.seqNumber, bufferMetaData.originId);
    NES_DEBUG("newGlobalWatermarkBuild {} for origin {} bufferMetaData {} ",
              newGlobalWatermark,
              getOutputOriginId(),
              bufferMetaData.toString());
    if (oldGlobalWatermark < newGlobalWatermark) {
        std::vector<IJIntervalPtr> intervalsToEmit;// Store intervals to emit after releasing lock
        {
            auto currentIntervalsLocked = currentIntervals.rlock();
            for (IJIntervalPtr interval : *currentIntervalsLocked) {
                // interval end is in future or interval is already emitted, skip this interval
                if (interval->getIntervalEnd() > static_cast<int64_t>(newGlobalWatermark)
                    || interval->intervalState == IJIntervalInfoState::EMITTED_TO_PROBE
                    || interval->intervalState == IJIntervalInfoState::MARKED_FOR_DELETION) {
                    NES_DEBUG(
                        "The interval {} can not be triggered yet or has already been triggered: interval timestamp {} vs {}",
                        interval->getId(),
                        interval->getIntervalEnd(),
                        static_cast<int64_t>(newGlobalWatermark));
                    continue;
                }
                interval->intervalState = IJIntervalInfoState::EMITTED_TO_PROBE;
                // we can simply emit as interval is does not need to be combined with other intervals
                intervalsToEmit.push_back(interval);
            }
            NES_DEBUG("Emitted {} of in total {} intervals", intervalsToEmit.size(), currentIntervalsLocked->size());
            currentIntervalsLocked.unlock();
        }

        for (auto& interval : intervalsToEmit) {
            emitIntervalToProbe(interval, pipelineCtx);
        }
    }
}

std::optional<IJIntervalPtr> IJOperatorHandler::getIntervalByIntervalIdentifier(uint64_t intervalIdentifier) {
    {
        auto currentIntervalLocked = currentIntervals.rlock();
        for (auto& curInterval : *currentIntervalLocked) {
            if (curInterval->getId() == intervalIdentifier) {
                return curInterval;
            }
        }
        currentIntervalLocked.unlock();
    }
    return std::nullopt;
}

// only called on stop of Handler
void IJOperatorHandler::triggerAllIntervals(PipelineExecutionContext* pipelineCtx) {
    std::vector<IJIntervalPtr> intervalsToEmit;// Store intervals to emit after releasing lock
    {
        auto currentIntervalLocked = currentIntervals.rlock();
        for (IJIntervalPtr interval : *currentIntervalLocked) {
            NES_DEBUG("interval {} is tested for triggering", interval->getId())
            switch (interval->intervalState) {
                case IJIntervalInfoState::LEFT_SIDE_FILLED:
                    NES_DEBUG("interval state switches from LEFT_SIDE_FILLED TO ONCE_SEEN_DURING_TERMINATION ")
                    interval->intervalState = IJIntervalInfoState::ONCE_SEEN_DURING_TERMINATION;
                    break;
                case IJIntervalInfoState::EMITTED_TO_PROBE: break;
                case IJIntervalInfoState::ONCE_SEEN_DURING_TERMINATION: {
                    NES_DEBUG("interval state switches from ONCE_SEEN_DURING_TERMINATION TO EMITTED_TO_PROBE")
                    interval->intervalState = IJIntervalInfoState::EMITTED_TO_PROBE;
                    intervalsToEmit.push_back(interval);
                    break;
                }
                case IJIntervalInfoState::MARKED_FOR_DELETION: break;
            }
        }
        currentIntervalLocked.unlock();
    }

    for (auto& interval : intervalsToEmit) {
        emitIntervalToProbe(interval, pipelineCtx);
    }

    // Lock for cleaning current interval vector from intervals with State 'MARKED_FOR_DELETION', i.e., remove the element using erase function and iterators
    {
        auto currentIntervalWLocked = currentIntervals.wlock();
        for (auto it = currentIntervalWLocked->begin(); it != currentIntervalWLocked->end();) {
            if ((*it)->intervalState == IJIntervalInfoState::MARKED_FOR_DELETION) {
                NES_INFO("Removing interval {}", (*it)->toString());
                it = currentIntervalWLocked->erase(it);// erase() returns next valid iterator
            } else {
                ++it;
            }
        }
        currentIntervalWLocked.unlock();
    }
}

void IJOperatorHandler::deleteAllIntervals() {
    {
        auto currentIntervalsLocked = currentIntervals.wlock();
        currentIntervalsLocked->clear();
        currentIntervalsLocked.unlock();
    }
    {
        auto rightTuplesLocked = rightTuples.wlock();
        rightTuplesLocked->clear();
        rightTuplesLocked.unlock();
    }
}

void IJOperatorHandler::deleteAllProcessedIntervals(BufferMetaData bufferMetaData) {
    uint64_t oldGlobalWaterMarkProbe = watermarkProcessorProbe->getCurrentWatermark();
    uint64_t newGlobalWaterMarkProbe =
        watermarkProcessorProbe->updateWatermark(bufferMetaData.watermarkTs, bufferMetaData.seqNumber, bufferMetaData.originId);
    NES_DEBUG("newGlobalWaterMarkProbe {} bufferMetaData {}", newGlobalWaterMarkProbe, bufferMetaData.toString());
    if (oldGlobalWaterMarkProbe < newGlobalWaterMarkProbe) {
        {
            auto currentIntervalWLocked = currentIntervals.wlock();
            for (auto it = currentIntervalWLocked->begin(); it != currentIntervalWLocked->end();) {
                if ((*it)->intervalState == IJIntervalInfoState::MARKED_FOR_DELETION) {
                    NES_DEBUG("Removing interval marked for deletion {}", (*it)->toString());
                    it = currentIntervalWLocked->erase(it);// erase() returns next valid iterator
                } else {
                    ++it;
                }
            }
            currentIntervalWLocked.unlock();
        }
    }
}

void IJOperatorHandler::stop(QueryTerminationType queryTerminationType, PipelineExecutionContextPtr pipelineCtx) {
    NES_INFO("Stopped IJOperatorHandler with {}!", magic_enum::enum_name(queryTerminationType));
    if (queryTerminationType == QueryTerminationType::Graceful) {
        triggerAllIntervals(pipelineCtx.get());
    }
}

void IJOperatorHandler::setBufferManager(const NES::Runtime::BufferManagerPtr& bufManager) { this->bufferManager = bufManager; }
IJOperatorHandlerPtr IJOperatorHandler::create(const std::vector<OriginId>& inputOrigins,
                                               const OriginId outputOriginId,
                                               const int64_t lowerBound,
                                               const int64_t upperBound,
                                               const SchemaPtr& leftSchema,
                                               const SchemaPtr& rightSchema,
                                               const uint64_t pageSizeLeft,
                                               const uint64_t pageSizeRight) {
    return std::make_shared<IJOperatorHandler>(inputOrigins,
                                               outputOriginId,
                                               lowerBound,
                                               upperBound,
                                               leftSchema,
                                               rightSchema,
                                               pageSizeLeft,
                                               pageSizeRight);
}

void IJOperatorHandler::updateRightTuples() {
    {
        auto rightTuplesLocked = rightTuples.wlock();
        NES_DEBUG("right tuple vector with {} page, remove {} tuples",
                  rightTuplesLocked->size(),
                  rightTuplesLocked->at(0)->getNumberOfEntries())
        rightTuplesLocked->clear();
        if (!updatedRightTuples.empty()) {
            NES_DEBUG("add {} page vector with {} tuples", updatedRightTuples.size(), updatedRightTuples[0]->getNumberOfEntries())
            rightTuplesLocked->push_back(std::move(updatedRightTuples[0]));
        } else {
            rightTuplesLocked->emplace_back(
                std::make_unique<Nautilus::Interface::PagedVectorVarSized>(bufferManager, getRightSchema(), getPageSizeRight()));
        }
        rightTuplesLocked.unlock();
    }
    updatedRightTuples.clear();
}

void* IJOperatorHandler::getUpdatedRightTuples() {
    if (updatedRightTuples.empty()) {
        updatedRightTuples.emplace_back(
            std::make_unique<Nautilus::Interface::PagedVectorVarSized>(bufferManager, getRightSchema(), getPageSizeRight()));
    }
    return updatedRightTuples[0].get();
}

uint64_t IJOperatorHandler::getWatermarkProbe() {
    NES_DEBUG("Get watermarkProbe {}", watermarkProcessorProbe->getCurrentWatermark())
    return watermarkProcessorProbe->getCurrentWatermark();
}
BufferManagerPtr IJOperatorHandler::getBufferManager() { return bufferManager; }
uint64_t IJOperatorHandler::getPageSizeRight() const { return pageSizeRight; }
int64_t IJOperatorHandler::getUpperBound() const { return upperBound; };
int64_t IJOperatorHandler::getLowerBound() const { return lowerBound; }
OriginId IJOperatorHandler::getOutputOriginId() const { return outputOriginId; }
uint64_t IJOperatorHandler::getNextSequenceNumber() { return sequenceNumber++; }
std::shared_ptr<IJInterval> IJOperatorHandler::getInterval(uint64_t index) { return currentIntervals->at(index); }
SchemaPtr& IJOperatorHandler::getRightSchema() { return rightSchema; }
}// namespace NES::Runtime::Execution::Operators
