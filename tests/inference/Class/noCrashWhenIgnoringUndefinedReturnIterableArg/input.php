<?php
function bar(iterable $_i) : void {}
function foo() : D {
    return new D();
}
bar(foo());
