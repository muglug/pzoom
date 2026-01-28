<?php
class Foo {
    const DIR = __DIR__;
}
$l = ["a", "b"];
$k = [Foo::DIR];
$a = implode("", $l);
$b = implode("", $k);
