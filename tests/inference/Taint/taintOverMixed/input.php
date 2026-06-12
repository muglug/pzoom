<?php
/**
 * @psalm-suppress MixedAssignment
 * @psalm-suppress MixedArgument
 */
function foo() : void {
    $a = $_GET["bad"];
    echo $a;
}
