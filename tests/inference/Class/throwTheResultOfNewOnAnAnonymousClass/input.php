<?php
declare(strict_types=1);

$test = new class extends \Exception { };
throw new $test();
