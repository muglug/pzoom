<?php
class Test1 {
    const VALUES = [
        "all",
        "own"
    ];
}

class Credentials {
    const ALL  = "all";
    const OWN  = "own";
    const NONE = "none";
}

class Test2 {
    const VALUES = [
        Credentials::ALL,
        Credentials::OWN
    ];
}

/**
 * @psalm-param list<"all"|"own"|"mine"> $value
 */
function test($value): void {
    print_r($value);
}

test(Test1::VALUES);
test(Test2::VALUES);
