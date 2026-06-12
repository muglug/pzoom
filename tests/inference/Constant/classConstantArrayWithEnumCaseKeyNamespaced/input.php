<?php
namespace OtherNamespace;
enum E: int {
    case K1 = 1;
    case K2 = 2;
}

namespace UsedNamespace;
enum E: int {
    case K3 = 3;
    case K4 = 4;
}

namespace AliasedNamespace;
enum E: int {
    case K5 = 5;
    case K6 = 6;
}

namespace SameNamespace;
use UsedNamespace\E;
use AliasedNamespace\E as E2;

enum E3: int {
    case K7 = 7;
    case K8 = 8;
}
class A {
    public const C = [
        \OtherNamespace\E::K1->name => "a",
        \OtherNamespace\E::K2->name => 10,
        \OtherNamespace\E::K1->value => "b",
        \OtherNamespace\E::K2->value => 11,
        E::K3->name => "c",
        E::K4->name => 12,
        E::K3->value => "d",
        E::K4->value => 13,
        E2::K5->name => "e",
        E2::K6->name => 14,
        E2::K5->value => "f",
        E2::K6->value => 15,
        E3::K7->name => "g",
        E3::K8->name => 16,
        E3::K7->value => "h",
        E3::K8->value => 17,
    ];
}
$c = A::C;
