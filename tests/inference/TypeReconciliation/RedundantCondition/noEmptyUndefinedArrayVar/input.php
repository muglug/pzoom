<?php
if (rand(0,1)) {
  /** @psalm-suppress UndefinedGlobalVariable */
  $a = $b[0];
} else {
  $a = null;
}
if ($a) {}