<?php

function example(string $s): void {
    if (preg_match('{foo-(\w+)}', $s, $m)) {
      /** @var array{string, string} $m */
      takesString($m[1]);
    }
}

function takesString(string $_s): void {}
