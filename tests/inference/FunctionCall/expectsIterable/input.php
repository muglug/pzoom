<?php
function foo(iterable $i) : void {}
function bar(array $a) : void {
    foo($a);
}
