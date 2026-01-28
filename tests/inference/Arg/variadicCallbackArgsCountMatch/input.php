<?php
/**
 * @param callable(string, string):void $callback
 * @return void
 */
function caller($callback) {}

/**
 * @param string ...$bar
 * @return void
 */
function foo(...$bar) {}

caller("foo");
