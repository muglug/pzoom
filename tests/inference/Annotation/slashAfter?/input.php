<?php
namespace ns;

/** @param ?\stdClass $_s */
function foo($_s) : void {
}

foo(null);
foo(new \stdClass);
