<?php
class MyTest {
    const LOOKUP = [
        "A00" => null,
        "A01" => null,
        "A02" => null,
        "A03" => null,
        "A04" => null,
        "A05" => null,
        "A06" => null,
        "A07" => null,
        "A010" => null,
        "A011" => null,
        "A012" => null,
        "A013" => null,
        "A014" => null,
        "A015" => null,
        "A016" => null,
        "A017" => null,
        "A020" => null,
        "A021" => null,
        "A022" => null,
        "A023" => null,
        "A024" => null,
        "A025" => null,
        "A026" => null,
        "A027" => null,
        "A030" => null,
        "A031" => null,
        "A032" => null,
        "A033" => null,
        "A034" => null,
        "A035" => null,
        "A036" => null,
        "A037" => null,
        "A040" => null,
        "A041" => null,
        "A042" => null,
        "A043" => null,
        "A044" => null,
        "A045" => null,
        "A046" => null,
        "A047" => null,
        "A050" => null,
        "A051" => null,
        "A052" => null,
        "A053" => null,
        "A054" => null,
        "A055" => null,
        "A056" => null,
        "A057" => null,
        "A060" => null,
        "A061" => null,
        "A062" => null,
        "A063" => null,
        "A064" => self::SUCCEED,
        "A065" => self::FAIL,
    ];

    const SUCCEED = "SUCCEED";
    const FAIL = "FAIL";

    /**
     * @param string $code
     */
    public static function will_succeed($code) : bool {
        // False positive TypeDoesNotContainType - string(SUCCEED) cannot be identical to null
        // This seems to happen because the array has a lot of entries.
        return (self::LOOKUP[strtoupper($code)] ?? null) === self::SUCCEED;
    }
}
