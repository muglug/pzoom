<?php
class C {}

/** @psalm-suppress UndefinedConstant */
$a = POTATO;

/** @psalm-suppress UndefinedConstant */
$a = C::POTATO;
