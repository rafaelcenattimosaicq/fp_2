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

#include <API/Schema.hpp>
#include <Common/PhysicalTypes/BasicPhysicalType.hpp>
#include <Exceptions/RuntimeException.hpp>
#include <Sources/Parsers/CSVParser.hpp>
#include <Util/Common.hpp>
#include <Util/Logger/Logger.hpp>
#include <Util/TestTupleBuffer.hpp>
#include <string>

using namespace std::string_literals;
namespace NES {

CSVParser::CSVParser(uint64_t numberOfSchemaFields, std::vector<NES::PhysicalTypePtr> physicalTypes, std::string delimiter)
    : Parser(physicalTypes), numberOfSchemaFields(numberOfSchemaFields), physicalTypes(std::move(physicalTypes)),
      delimiter(std::move(delimiter)) {}

bool CSVParser::writeInputTupleToTupleBuffer(std::string_view csvInputLine,
                                             uint64_t tupleCount,
                                             Runtime::MemoryLayouts::TestTupleBuffer& tupleBuffer,
                                             const SchemaPtr& schema,
                                             const Runtime::BufferManagerPtr& bufferManager) {
    NES_TRACE("CSVParser::parseCSVLine: Current TupleCount:  {}", tupleCount);

    std::vector<std::string> values;
    try {
        values = NES::Util::splitWithStringDelimiter<std::string>(csvInputLine, delimiter);
    } catch (std::exception e) {
        NES_WARNING("CSVParser::writeInputTupleToTupleBuffer: Split failed for line '{}': {}", csvInputLine, e.what());
        return false;
    }

    if (values.size() != schema->getSize()) {
        NES_WARNING("CSVParser: Field count mismatch — schema expects {} fields but got {} from line: {}",
                    schema->getSize(), values.size(), csvInputLine);
        return false;
    }
    // iterate over fields of schema and cast string values to correct type
    for (uint64_t j = 0; j < numberOfSchemaFields; j++) {
        auto field = physicalTypes[j];
        NES_TRACE("Current value is:  {}", values[j]);
        writeFieldValueToTupleBuffer(values[j], j, tupleBuffer, schema, tupleCount, bufferManager);
    }
    return true;
}
}// namespace NES
