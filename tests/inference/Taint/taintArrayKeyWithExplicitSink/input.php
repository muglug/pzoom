<?php
/** @psalm-taint-sink html $values */
function doTheMagic(array $values) {}

doTheMagic([(string)$_GET["bad"] => "foo"]);
