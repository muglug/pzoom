<?php
function takesInt(int $i) : void {
    echo $i;
}

function takesString(string $s) : void {
    echo $s;
}

/**
 * @return Generator<int, string, mixed, int>
 */
function other_generator() : Generator {
    yield "traffic";
    return 1;
}

/**
 * @return Generator<int, string>
 */
function foo() : Generator {
    $a = yield from other_generator();
    takesInt($a);
}

foreach (foo() as $s) {
    takesString($s);
}
