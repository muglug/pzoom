<?php
function add(int $a, int $b, int ...$rest): int
{
    return 0;
}

/** @param Closure(int, int, string, int, int): int $f */
function int_int_string_int_int(Closure $f): void {}

/** @param Closure(int, int, int, string, int): int $f */
function int_int_int_string_int(Closure $f): void {}

/** @param Closure(int, int, int, int, string): int $f */
function int_int_int_int_string(Closure $f): void {}

int_int_string_int_int(add(...));
int_int_int_string_int(add(...));
int_int_int_int_string(add(...));
