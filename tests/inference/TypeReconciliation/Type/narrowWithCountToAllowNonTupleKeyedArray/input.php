<?php
/**
 * @param list<string> $arr
 */
function foo($arr): void {
    if (count($arr) === 2) {
        consume($arr);
    }
}

/**
 * @param array{0:string, 1: string} $input
 */
function consume($input): void{}