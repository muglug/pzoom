<?php
class Exists {}
function bar(Exists $_i) : void {}
function foo() : D {
    return new D();
}
bar(foo());
