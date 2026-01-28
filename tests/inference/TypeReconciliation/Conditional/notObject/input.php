<?php
function f(): ?object {
      return rand(0,1) ? new stdClass : null;
}

$data = f();
if (!$data) {}
if ($data) {}
