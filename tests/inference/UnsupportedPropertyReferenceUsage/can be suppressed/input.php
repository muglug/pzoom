<?php
class A {
    public int $b = 0;
}
$a = new A();
/** @psalm-suppress UnsupportedPropertyReferenceUsage */
$b = &$a->b;
