<?php
/**
 * @param callable(string, string, string, string=):bool $arg
 * @return void
 */
function foo($arg) {}

function bar(string $a, string $b, string $c, string $d = ""): bool {}

foo("bar");

/**
 * @param callable(string, string, string):bool $arg
 * @return void
 */
function foo1($arg) {}

function bar1(string $a, string $b, string $c, string $d = ""): bool {}

foo1("bar1");
