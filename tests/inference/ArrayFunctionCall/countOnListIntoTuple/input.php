<?php
/** @param array{string, string} $tuple */
function foo(array $tuple) : void {}

/** @param list<string> $list */
function bar(array $list) : void {
    if (count($list) === 2) {
        foo($list);
    }
}
