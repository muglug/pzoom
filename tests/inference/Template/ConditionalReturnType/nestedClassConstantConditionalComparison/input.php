<?php
class A {
    const TYPE_STRING = 0;
    const TYPE_INT = 1;

    /**
     * @template T as int
     * @param T $i
     * @psalm-return (
     *     T is self::TYPE_STRING
     *     ? string
     *     : (T is self::TYPE_INT ? int : bool)
     * )
     */
    public static function getDifferentType(int $i) {
        if ($i === self::TYPE_STRING) {
            return "hello";
        }

        if ($i === self::TYPE_INT) {
            return 5;
        }

        return true;
    }
}

$string = A::getDifferentType(0);
$int = A::getDifferentType(1);
$bool = A::getDifferentType(4);
$string2 = (new A)->getDifferentType(0);
$int2 = (new A)->getDifferentType(1);
$bool2 = (new A)->getDifferentType(4);