<?php
class A {
    const T1 = 1;
    const T2 = 2;

    /**
     * @param self::T* $t
     */
    public static function bar(int $t):void {}

    /**
     * @psalm-assert-if-true self::T* $t
     */
    public static function isValid(int $t): bool {
        return in_array($t, [self::T1, self::T2], true);
    }
}

function takesA(int $a) : void {
    if (A::isValid($a)) {
        A::bar($a);
    }
}
