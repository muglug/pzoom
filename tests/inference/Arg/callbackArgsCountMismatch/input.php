<?php
/**
 * @param callable(string, string):void $callback
 * @return void
 */
function caller($callback) {}

/**
 * @param string $a
 * @return void
 */
function foo($a) {}

caller("foo");
