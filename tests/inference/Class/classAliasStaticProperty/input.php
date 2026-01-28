<?php
class A {
    /** @var int */
    public static $prop = 1;
}
class_alias(A::class, B::class);
B::$prop = 123;
