<?php
$y = date("Y");
$ym = date("Ym");
$y_m = date("Y-m");
$m = date("m");
$F = date("F");
$y2 = date("Y", 10000);
$F2 = date("F", 10000);
/** @psalm-suppress MixedArgument */
$F3 = date("F", $GLOBALS["F3"]);
$gm_y = gmdate("Y");
$gm_ym = gmdate("Ym");
$gm_m = gmdate("m");
$gm_F = gmdate("F");
$gm_y2 = gmdate("Y", 10000);
$gm_F2 = gmdate("F", 10000);
/** @psalm-suppress MixedArgument */
$gm_F3 = gmdate("F", $GLOBALS["F3"]);
