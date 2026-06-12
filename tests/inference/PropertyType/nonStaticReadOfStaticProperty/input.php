<?php
class A {
    /** @var int */
    public static $prop = 1;
}
echo (new A)->prop;
