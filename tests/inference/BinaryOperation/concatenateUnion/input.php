<?php
$arr = ["foobar" => false, "foobaz" => true, "barbaz" => true];
$foo = random_int(0, 1) ? "foo" : "bar";
$foo .= "baz";
$val = $arr[$foo];
