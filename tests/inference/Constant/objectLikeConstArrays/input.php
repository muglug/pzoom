<?php
class C {
    const A = 0;
    const B = 1;

    const ARR = [
        self::A => "zero",
        self::B => "two",
    ];
}

if (C::ARR[C::A] === "two") {}
