<?php
    $e = new Exception(); // necessary to trick Psalm’s scanner for test
    /** @var Exception $this */
?>
<h1><?= $this->getMessage() ?></h1>
