<?php
function foo() : void {
    /**
     * @psalm-suppress TooManyArguments
     * here
     */
    echo strlen("a", "b");
}
