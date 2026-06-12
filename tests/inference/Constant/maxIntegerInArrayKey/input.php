<?php
class A {
    // PHP_INT_MAX
    public const S = ['9223372036854775807' => 1];
    public const I = [9223372036854775807 => 1];

    // PHP_INT_MAX + 1
    public const SO = ['9223372036854775808' => 1];
}
$s = A::S;
$i = A::I;
$so = A::SO;
