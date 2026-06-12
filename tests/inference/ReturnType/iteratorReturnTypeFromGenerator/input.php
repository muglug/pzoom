<?php
function foo1(): Generator {
    foreach ([1, 2, 3] as $i) {
        yield $i;
    }
}

function foo2(): Iterator {
    foreach ([1, 2, 3] as $i) {
        yield $i;
    }
}

function foo3(): Traversable {
    foreach ([1, 2, 3] as $i) {
        yield $i;
    }
}

function foo4(): iterable {
    foreach ([1, 2, 3] as $i) {
        yield $i;
    }
}

foreach (foo1() as $i) echo $i;
foreach (foo2() as $i) echo $i;
foreach (foo3() as $i) echo $i;
foreach (foo4() as $i) echo $i;
