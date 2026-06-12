<?php
class B {
    const B = 4;
}
class A {
    const B = "four";
    const C = [
        B::B => "one",
    ];
}

echo A::C[4];
