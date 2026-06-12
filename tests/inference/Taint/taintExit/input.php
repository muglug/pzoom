<?php
if (rand(0, 1)) {
    /** @psalm-suppress PossiblyInvalidArgument */
    exit($_GET['a']);
} else {
    /** @psalm-suppress PossiblyInvalidArgument */
    die($_GET['b']);
}
