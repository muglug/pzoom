<?php
enum E {
    case K1;
    case K2;
}
enum BEI: int {
    case K3 = 1;
    case K4 = 2;
}
enum BES: string {
    case K5 = "a";
    case K6 = "b";
}
class A {
    public const C = [
        BEI::K3->value => "e",
        BEI::K4->value => 5,
        E::K1->name => "c",
        E::K2->name => 3,
        BEI::K3->name => "d",
        BEI::K4->name => 4,
        BES::K5->name => "f",
        BES::K6->name => 6,
        BES::K5->value => "g",
        BES::K6->value => 7,
    ];
}
$c = A::C;
