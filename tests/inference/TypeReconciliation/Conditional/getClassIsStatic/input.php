<?php
class A {}

class AChild extends A {
    public static function compare(A $other_type) : AChild {
        if (get_class($other_type) !== static::class) {
            throw new \Exception();
        }

        return $other_type;
    }
}