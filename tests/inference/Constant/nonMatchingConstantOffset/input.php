<?php
class A {
    const KEYS = ["one", "two", "three", "four"];
    const ARR = [
        "one" => 1,
        "two" => 2
    ];

    const ARR2 = [
        "three" => 3,
        "four" => 4
    ];
}

foreach (A::KEYS as $key) {
    if (isset(A::ARR[$key])) {
        echo A::ARR2[$key];
    }
}
