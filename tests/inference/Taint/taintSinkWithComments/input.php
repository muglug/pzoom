<?php

/**
 * @psalm-taint-sink html $sink
 *
 * Not working
 */
function sinkNotWorking($sink) : string {}

echo sinkNotWorking($_GET["taint"]);
