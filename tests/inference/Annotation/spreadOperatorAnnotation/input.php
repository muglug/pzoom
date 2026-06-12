<?php
/** @param string[] $_s */
function foo(string ...$_s) : void {}
/** @param string ...$_s */
function bar(string ...$_s) : void {}
foo("hello", "goodbye");
bar("hello", "goodbye");
foo(...["hello", "goodbye"]);
bar(...["hello", "goodbye"]);
