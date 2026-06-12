<?php
function unsafe() {
    /**
     * @psalm-suppress TaintedInput
     */
    echo $_GET["x"];
}
