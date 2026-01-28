<?php
/** @psalm-suppress MixedAssignment */
$a = $GLOBALS["foo"];

/** @psalm-suppress MixedArrayAssignment */
$a["bar"] = "cool";

/** @psalm-suppress MixedMethodCall */
$a->offsetExists("baz");
