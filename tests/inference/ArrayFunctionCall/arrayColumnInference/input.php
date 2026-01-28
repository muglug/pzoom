<?php
function makeMixedArray(): array { return []; }
/** @return array<array<int,bool>> */
function makeGenericArray(): array { return []; }
/** @return array<array{0:string}> */
function makeShapeArray(): array { return []; }
/** @return array<array{0:string}|int> */
function makeUnionArray(): array { return []; }
/** @return array<string, array{x?:int, y?:int, width?:int, height?:int}> */
function makeKeyedArray(): array { return []; }
$a = array_column([[1], [2], [3]], 0);
$b = array_column([["a" => 1], ["a" => 2], ["a" => 3]], "a");
$c = array_column([["a" => 1], ["a" => 2], ["a" => 3]], null, "a");
$d = array_column([["a" => 1], ["a" => 2], ["a" => 3]], null, "b");
$e = array_column([["a" => 1], ["a" => 2], ["a" => 3]], rand(0,1) ? "a" : "b", "b");
$f = array_column([["k" => "a", "v" => 1], ["k" => "b", "v" => 2]], "v", "k");
$g = array_column([], 0);
$h = array_column(makeMixedArray(), 0);
$i = array_column(makeMixedArray(), 0, "k");
$j = array_column(makeMixedArray(), 0, null);
$k = array_column(makeGenericArray(), 0);
$l = array_column(makeShapeArray(), 0);
$m = array_column(makeUnionArray(), 0);
$n = array_column([[0 => "test"]], 0);
$o = array_column(makeKeyedArray(), "y");
$p_prepare = makeKeyedArray();
assert($p_prepare !== []);
$p = array_column($p_prepare, "y");
                
