<?php
$a = hrtime(true);
$b = hrtime();
/** @psalm-suppress InvalidScalarArgument */
$c = hrtime(1);
$d = hrtime(false);
