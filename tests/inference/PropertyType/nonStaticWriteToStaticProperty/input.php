<?php
class A {
    /** @var int */
    public static $prop = 1;
}
(new A)->prop = 42;
