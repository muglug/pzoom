<?php
enum E: string {
    case K1 = "a";
    case K2 = "b";
    case K3 = "c";
    case K4 = "d";
    case K5 = "e";
    case K6 = "f";
    case K7 = "g";
}
class A {
    public const C = [
        E::K1->name => [
            E::K2->name => [
                E::K3->name => "h",
                E::K4->name => "i",
            ],
            E::K5->name => [
                E::K6->name => "j",
                E::K7->name => "k",
            ],
        ],
        E::K1->value => [
            E::K2->value => [
                E::K3->value => "l",
                E::K4->value => "m",
            ],
            E::K5->value => [
                E::K6->value => "n",
                E::K7->value => "o",
            ],
        ]
    ];
}
$c = A::C;
