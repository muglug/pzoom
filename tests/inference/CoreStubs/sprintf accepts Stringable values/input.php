<?php

$a = sprintf(
    "%s",
    new class implements Stringable { public function __toString(): string { return "hello"; } },
);
