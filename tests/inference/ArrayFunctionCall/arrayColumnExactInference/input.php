<?php
$a = array_column([
    ["v" => "a"],
    ["v" => "b"],
    ["v" => "c"],
    ["v" => "d"],
], "v");

$b = array_column([
    ["v" => "a"],
    [],
    ["v" => "c"],
    ["v" => "d"],
], "v");

$c = array_column([
    ["v" => "a"],
    123,
    ["v" => "c"],
    ["v" => "d"],
], "v");

$d = array_column([
    ["v" => "a", "k" => "A"],
    ["v" => "b", "k" => "B"],
    ["v" => "c", "k" => "C"],
    ["v" => "d", "k" => "D"],
], "v", "k");

$e = array_column([
    ["v" => "a", "k" => 0],
    ["v" => "b", "k" => 1],
    ["v" => "c", "k" => 2],
    ["v" => "d", "k" => 3],
], "v", "k");

$f = array_column([
    ["v" => "a", "k" => 3],
    ["v" => "b", "k" => 2],
    ["v" => "c", "k" => 1],
    ["v" => "d", "k" => 0],
], "v", "k");

$g = array_column([
    ["v" => "a", "k" => 0],
    ["v" => "b", "k" => 1],
    ["v" => "c", "k" => 2],
    ["v" => "d", "k" => 3],
], null, "k");

$h = array_column([
    "a" => ["k" => 0],
    "b" => ["k" => 1],
    "c" => ["k" => 2],
], null, "k");

/** @var array{a: array{v: 0}, b?: array{v: 1}} */
$aa = [];
$i = array_column($aa, "v");

/** @var array{a: array{v: "a", k: 0}, b?: array{v: "b", k: 1}, c: array{v: "c", k: 2}} */
$aa = [];
$j = array_column($aa, null, "k");

/** @var array{a: array{v: "a", k: 0}, b: array{v: "b", k: 1}, c?: array{v: "c", k: 2}} */
$aa = [];
$k = array_column($aa, null, "k");

$l = array_column(["test" => ["v" => "a"], "test2" => ["v" => "b"]], "v");
                
