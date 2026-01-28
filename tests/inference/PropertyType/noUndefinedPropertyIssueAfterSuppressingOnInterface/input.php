<?php
interface I {}

function bar(I $i) : void {
    /**
     * @psalm-suppress NoInterfaceProperties
     * @psalm-suppress MixedArgument
     */
    echo $i->foo;
}
