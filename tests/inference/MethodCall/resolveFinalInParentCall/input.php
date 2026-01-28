<?php
abstract class A {
    protected static function create() : static {
        return new static();
    }

    final private function __construct() {}
}

final class B extends A {
    public static function new() : self {
        return parent::create();
    }
}
