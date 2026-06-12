<?php
class A {
    public const I = [
        9223372036854775806 => 0,
        1, // expected key = PHP_INT_MAX
    ];
}
$s = A::I;
