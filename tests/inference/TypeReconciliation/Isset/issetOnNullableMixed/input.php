<?php
function processParam(mixed $param) : void {
    if (rand(0, 1)) {
        $param = null;
    }

    if (isset($param["name"])) {
        /**
         * @psalm-suppress MixedArgument
         */
        echo $param["name"];
    }
}