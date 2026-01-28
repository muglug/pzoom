<?php
trait T {
    public static function getSelf(): self {
        return new self();
    }

    public static function callGetSelf(): self {
        return self::getSelf();
    }
}

class A {
    use T;
}

class B {
    use T;
}
