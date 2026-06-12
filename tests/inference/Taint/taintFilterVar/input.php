<?php
/** @psalm-taint-sink input_except_sleep $value */
function taintSink(mixed $value): void {}

taintSink(filter_var($_GET["bad"], FILTER_VALIDATE_INT));
taintSink(filter_var($_GET["bad"], FILTER_VALIDATE_BOOLEAN));
taintSink(filter_var($_GET["bad"], FILTER_VALIDATE_FLOAT));
taintSink(filter_var($_GET["bad"], FILTER_SANITIZE_NUMBER_INT));
taintSink(filter_var($_GET["bad"], FILTER_SANITIZE_NUMBER_FLOAT));

