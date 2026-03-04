
// Generated from CLionProjects/nebulastream/nes-coordinator/src/Parsers/NebulaPSL/gen/NesCEP.g4 by ANTLR 4.9.2

#ifndef NES_COORDINATOR_INCLUDE_PARSERS_NEBULAPSL_GEN_NESCEPLEXER_H_
#define NES_COORDINATOR_INCLUDE_PARSERS_NEBULAPSL_GEN_NESCEPLEXER_H_
#pragma once

#include <antlr4-runtime/antlr4-runtime.h>

namespace NES::Parsers {

class NesCEPLexer : public antlr4::Lexer {
  public:
    enum {
        T__0 = 1,
        T__1 = 2,
        T__2 = 3,
        T__3 = 4,
        T__4 = 5,
        T__5 = 6,
        T__6 = 7,
        T__7 = 8,
        T__8 = 9,
        T__9 = 10,
        T__10 = 11,
        T__11 = 12,
        T__12 = 13,
        T__13 = 14,
        T__14 = 15,
        T__15 = 16,
        T__16 = 17,
        T__17 = 18,
        T__18 = 19,
        T__19 = 20,
        T__20 = 21,
        INT = 22,
        FLOAT = 23,
        PROTOCOL = 24,
        FILETYPE = 25,
        PORT = 26,
        WS = 27,
        FROM = 28,
        PATTERN = 29,
        WHERE = 30,
        WITHIN = 31,
        CONSUMING = 32,
        SELECT = 33,
        INTO = 34,
        ORDER = 35,
        ALL = 36,
        ANY = 37,
        SEP = 38,
        COMMA = 39,
        LPARENTHESIS = 40,
        RPARENTHESIS = 41,
        NOT = 42,
        NOT_OP = 43,
        SEQ = 44,
        NEXT = 45,
        AND = 46,
        OR = 47,
        STAR = 48,
        PLUS = 49,
        D_POINTS = 50,
        LBRACKET = 51,
        RBRACKET = 52,
        XOR = 53,
        IN = 54,
        IS = 55,
        NULLTOKEN = 56,
        BETWEEN = 57,
        BINARY = 58,
        TRUE = 59,
        FALSE = 60,
        UNKNOWN = 61,
        QUARTER = 62,
        MONTH = 63,
        DAY = 64,
        HOUR = 65,
        MINUTE = 66,
        WEEK = 67,
        SECOND = 68,
        MICROSECOND = 69,
        AS = 70,
        EQUAL = 71,
        KAFKA = 72,
        FILE = 73,
        MQTT = 74,
        NETWORK = 75,
        NULLOUTPUT = 76,
        OPC = 77,
        PRINT = 78,
        ZMQ = 79,
        POINT = 80,
        QUOTE = 81,
        AVG = 82,
        SUM = 83,
        MIN = 84,
        MAX = 85,
        COUNT = 86,
        IF = 87,
        LOGOR = 88,
        LOGAND = 89,
        LOGXOR = 90,
        NONE = 91,
        URL = 92,
        NAME = 93,
        ID = 94,
        PATH = 95
    };

    explicit NesCEPLexer(antlr4::CharStream* input);
    ~NesCEPLexer();

    virtual std::string getGrammarFileName() const override;
    virtual const std::vector<std::string>& getRuleNames() const override;

    virtual const std::vector<std::string>& getChannelNames() const override;
    virtual const std::vector<std::string>& getModeNames() const override;
    virtual const std::vector<std::string>& getTokenNames() const override;// deprecated, use vocabulary instead
    virtual antlr4::dfa::Vocabulary& getVocabulary() const override;

    virtual const std::vector<uint16_t> getSerializedATN() const override;
    virtual const antlr4::atn::ATN& getATN() const override;

  private:
    static std::vector<antlr4::dfa::DFA> _decisionToDFA;
    static antlr4::atn::PredictionContextCache _sharedContextCache;
    static std::vector<std::string> _ruleNames;
    static std::vector<std::string> _tokenNames;
    static std::vector<std::string> _channelNames;
    static std::vector<std::string> _modeNames;

    static std::vector<std::string> _literalNames;
    static std::vector<std::string> _symbolicNames;
    static antlr4::dfa::Vocabulary _vocabulary;
    static antlr4::atn::ATN _atn;
    static std::vector<uint16_t> _serializedATN;

    // Individual action functions triggered by action() above.

    // Individual semantic predicate functions triggered by sempred() above.

    struct Initializer {
        Initializer();
    };
    static Initializer _init;
};

}// namespace NES::Parsers
#endif// NES_COORDINATOR_INCLUDE_PARSERS_NEBULAPSL_GEN_NESCEPLEXER_H_
