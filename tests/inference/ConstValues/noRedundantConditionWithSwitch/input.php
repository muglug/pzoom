<?php
namespace Ns;

/**
 * @psalm-param ( "foo" | "bar") $s
 */
function foo(string $s) : void {
    switch ($s) {
      case "foo":
        break;
      case "bar":
        break;
    }
}
