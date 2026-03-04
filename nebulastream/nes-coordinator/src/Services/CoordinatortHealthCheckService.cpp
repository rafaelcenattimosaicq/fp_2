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

#include <Catalogs/Topology/Topology.hpp>
#include <Configurations/Coordinator/CoordinatorConfiguration.hpp>
#include <GRPC/WorkerRPCClient.hpp>
#include <Services/CoordinatorHealthCheckService.hpp>
#include <Util/Logger/Logger.hpp>
#include <Util/ThreadNaming.hpp>
#include <vector>

namespace NES {

CoordinatorHealthCheckService::CoordinatorHealthCheckService(TopologyPtr topology,
                                                             std::string healthServiceName,
                                                             Configurations::CoordinatorConfigurationPtr coordinatorConfiguration)
    : topology(topology), workerRPCClient(WorkerRPCClient::create()), coordinatorConfiguration(coordinatorConfiguration) {
    id = 9999;
    this->healthServiceName = healthServiceName;
}

void CoordinatorHealthCheckService::startHealthCheck() {
    NES_INFO("CoordinatorHealthCheckService::startHealthCheck");
    isRunning = true;
    NES_DEBUG("start health checking on coordinator");
    healthCheckingThread = std::make_shared<std::thread>(([this]() {
        setThreadName("nesHealth");
        auto waitTime = std::chrono::seconds(this->coordinatorConfiguration->coordinatorHealthCheckWaitTime.getValue());
        while (isRunning) {
            // Collect dead worker IDs during iteration so we can remove them
            // from the health check map after releasing the locked_table.
            // Erasing from cuckoohash_map while lock_table() is alive would
            // deadlock because lock_table holds all bucket locks.
            std::vector<WorkerId> deadWorkerIds;
            bool rootNodeFailed = false;
            {
                auto lockedTable = topologyIdToRPCAddressMap.lock_table();
                for (auto const& node : lockedTable) {
                    std::string destAddress = node.second;
                    //check health
                    NES_INFO("NesCoordinator::healthCheck: checking node= {}", destAddress);
                    auto res = workerRPCClient->checkHealth(destAddress, healthServiceName);
                    if (res) {
                        NES_TRACE("NesCoordinator::healthCheck: node={} is alive", destAddress);
                        if (inactiveWorkers.contains(node.first)) {
                            inactiveWorkers.erase(node.first);
                        }
                    } else {
                        NES_WARNING("NesCoordinator::healthCheck: node={} went dead so we remove it", destAddress);
                        const auto& rootWorkerNodeIds = topology->getRootWorkerNodeIds();
                        auto found = std::find(rootWorkerNodeIds.begin(), rootWorkerNodeIds.end(), node.first);
                        if (found != rootWorkerNodeIds.end()) {
                            NES_WARNING("The failing node is the root node so we cannot delete it");
                            rootNodeFailed = true;
                            break;
                        } else {
                            deadWorkerIds.push_back(node.first);
                        }
                    }
                }
            } // locked_table released here

            if (rootNodeFailed) {
                shutdownRPC->set_value(true);
                return;
            }

            // Now remove dead nodes from topology and health check map.
            for (auto const& deadId : deadWorkerIds) {
                auto ret = topology->unregisterWorker(deadId);
                inactiveWorkers.insert(deadId);
                // Remove the dead node from the health check map so we don't
                // keep polling a stale address forever.  Without this erase
                // the entry survives across worker restarts, causing infinite
                // "node went dead" log spam and preventing the re-registered
                // worker from being health-checked at its new address.
                topologyIdToRPCAddressMap.erase(deadId);
                if (ret) {
                    NES_TRACE("NesCoordinator::healthCheck: remove node={} successfully", deadId);
                } else {
                    NES_WARNING("Node went offline but could not be removed from topology id={}", deadId);
                }
            }
            {
                std::unique_lock<std::mutex> lk(cvMutex);
                cv.wait_for(lk, waitTime, [this] {
                    return isRunning == false;
                });
            }
        }
        shutdownRPC->set_value(true);
        NES_DEBUG("NesCoordinator: stop health checking");
    }));
}

}// namespace NES
