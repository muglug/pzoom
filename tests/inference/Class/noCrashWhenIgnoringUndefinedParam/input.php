<?php
function bar(iterable $_i) : void {}
function foo(C $c) : void {
    bar($c);
}
