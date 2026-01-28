<?php
/** @param array<mixed, int|string> $foo */
function fooFoo(array $foo): void { }

function barBar(array $bar): void {
    fooFoo($bar);
}

barBar([1, "2"]);
