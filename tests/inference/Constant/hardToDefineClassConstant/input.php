<?php
class A {
    const C = [
        self::B => 4,
        "name" => 3
    ];

    const B = 4;
}

echo A::C[4];
