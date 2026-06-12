<?php
class X {
    public const A = 1;
    public const B = 2;
    public const C = self::A & self::B;
}

$c = X::C;
