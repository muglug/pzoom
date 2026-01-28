<?php
class C {
    const ONE = 1;
    const TWO = self::ONE * 2;
    const THREE = self::TWO + 1;
    const ONE_THIRD = self::ONE / self::THREE;
    const SENTENCE = "The value of THREE is " . self::THREE;
    const SHIFT = self::ONE >> 2;
    const SHIFT2 = self::ONE << 1;
    const BITAND = 1 & 1;
    const BITOR = 1 | 1;
    const BITXOR = 1 ^ 1;

    /** @var int */
    public $four = self::ONE + self::THREE;

    /**
     * @param  int $a
     * @return int
     */
    public function f($a = self::ONE + self::THREE) {
        return $a;
    }
}

$c1 = C::ONE;
$c2 = C::TWO;
$c3 = C::THREE;
$c1_3rd = C::ONE_THIRD;
$c_sentence = C::SENTENCE;
$cf = (new C)->f();
$c4 = (new C)->four;
$shift = C::SHIFT;
$shift2 = C::SHIFT2;
$bitand = C::BITAND;
$bitor = C::BITOR;
$bitxor = C::BITXOR;
