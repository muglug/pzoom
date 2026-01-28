<?php
function foo(array $test) : void {
    /** @psalm-suppress MixedArgument */
    echo $test[0] ?? ( $test[0] = 1 );
    /** @psalm-suppress MixedArgument */
    echo $test[0];
}