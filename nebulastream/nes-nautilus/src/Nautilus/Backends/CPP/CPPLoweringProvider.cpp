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

#include <Nautilus/Backends/CPP/CPPLoweringContext.hpp>
#include <Nautilus/Backends/CPP/CPPLoweringProvider.hpp>

namespace NES::Nautilus::Backends::CPP {

std::string CPPLoweringProvider::lower(std::shared_ptr<IR::IRGraph> ir) {
    auto ctx = CPPLoweringContext(std::move(ir));
    return ctx.process().str();
}

}// namespace NES::Nautilus::Backends::CPP
