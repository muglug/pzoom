<?php
function foo() : bool {
    /** @psalm-suppress TooFewArguments */
    return count() > 0;
}