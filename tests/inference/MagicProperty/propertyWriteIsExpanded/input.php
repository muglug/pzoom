<?php
/** @property self::TYPE_* $type */
class A {
    public const TYPE_A = 1;
    public const TYPE_B = 2;

    public function __get(string $_prop) {}
    /** @param mixed $_value */
    public function __set(string $_prop, $_value) {}
}
$a = (new A);
$a->type = A::TYPE_B;
                
