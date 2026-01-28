<?php
/**
 * @param list<int> $_bar
 */
function foo(array $_bar) : void {}

/**
 * @param list<int> $bar
 */
function baz(array $bar) : void {
    foo((array) $bar);
}
