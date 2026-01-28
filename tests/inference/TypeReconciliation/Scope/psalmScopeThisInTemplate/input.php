<?php
    $e = new Exception(); // necessary to trick Psalm’s scanner for test
    /** @psalm-scope-this Exception */
?>
<h1><?= $this->getMessage() ?></h1>
