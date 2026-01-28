<?php
class A {}

trait T {
    private static ?A $one = null;

    private static function maybeSetOne(): A {
        if (null === self::$one) {
            self::$one = new A();
        }

        return self::$one;
    }
}

class Implementer {
    use T;
}