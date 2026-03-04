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

#ifndef NES_OPERATORS_INCLUDE_OPERATORS_LOGICALOPERATORS_LOGICALINTERVALJOINOPERATOR_HPP_
#define NES_OPERATORS_INCLUDE_OPERATORS_LOGICALOPERATORS_LOGICALINTERVALJOINOPERATOR_HPP_

#include <Operators/AbstractOperators/OriginIdAssignmentOperator.hpp>
#include <Operators/LogicalOperators/LogicalBinaryOperator.hpp>
#include <Operators/LogicalOperators/LogicalIntervalJoinDescriptor.hpp>
#include <memory>

namespace NES {

/**
* @brief Interval Join operator, which contains an expression as a predicate.
*/
class LogicalIntervalJoinOperator : public LogicalBinaryOperator, public OriginIdAssignmentOperator {
  public:
    explicit LogicalIntervalJoinOperator(Join::LogicalIntervalJoinDescriptorPtr intervalJoinDefinition,
                                         OperatorId id,
                                         OriginId originId = INVALID_ORIGIN_ID);
    ~LogicalIntervalJoinOperator() override = default;

    /**
    * @brief getter join definition.
    * @return LogicalJoinDescriptor
    */
    Join::LogicalIntervalJoinDescriptorPtr getIntervalJoinDefinition() const;

    /**
     * checks whether two interval join operators are equal by comparing their join definition, input and output schemas
     * @note the operator ids are not compared
     * @param rhs : join operator to compare with
     * @return true if equal
     */
    [[nodiscard]] bool equal(NodePtr const& rhs) const override;

    /**
     * checks if two interval join operators are identical
     * @note two operator are identical if they are equal and share one id
     * @param rhs : join operator to compare with
     * @return true if identical
     */
    [[nodiscard]] bool isIdentical(NodePtr const& rhs) const override;

    /**
     * creates a string representation of the operator containing the operator id
     * @return
     */
    [[nodiscard]] std::string toString() const override;

    /**
     * infers the schema of two child operators
     * @return true if no errors appeared
     */
    bool inferSchema() override;

    /**
     * infers the timestamp field in the timeCharacteristic
     * @return true if no errors appeared
     */
    bool inferTimeStamp();

    /**
     * creates a copy of the operator
     * @return pointer to the copy
     */
    OperatorPtr copy() override;

    void inferStringSignature() override;
    std::vector<OriginId> getOutputOriginIds() const override;
    void setOriginId(OriginId originId) override;
    const std::string& getIntervalStartFieldName() const;
    void setIntervalStartFieldName(const std::string& intervalStartFieldName);
    const std::string& getIntervalEndFieldName() const;
    void setIntervalEndFieldName(const std::string& intervalEndFieldName);
    /**
     * @brief Sets the window start, end, and key field name during the serialization of the operator
     * @param windowStartFieldName
     * @param windowEndFieldName
     */
    void setIntervalStartEndKeyFieldName(std::string_view intervalStartFieldName, std::string_view intervalEndFieldName);

    /**
     * @brief Sets the window start, end, and key field name during the serialization of the operator
     * @param joinKey a FieldAccessExpressionNode to find in the schemas
     * @param inputSchema the schema corresponding to the join side
     */
    bool findSchemaInDistinctSchemas(FieldAccessExpressionNode& joinKey, const SchemaPtr& inputSchema);

  private:
    Join::LogicalIntervalJoinDescriptorPtr intervalJoinDefinition;
    std::string intervalStartFieldName;
    std::string intervalEndFieldName;
};
}// namespace NES
#endif// NES_OPERATORS_INCLUDE_OPERATORS_LOGICALOPERATORS_LOGICALINTERVALJOINOPERATOR_HPP_
