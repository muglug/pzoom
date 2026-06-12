<?php
/** @param callable():?bool $c */
function takesCallable(callable $c) : void {}

takesCallable(function() { return; });
