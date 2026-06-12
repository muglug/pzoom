<?php

static fn (\UnitEnum $tag): string => $tag->name;

static fn (\BackedEnum $tag): string|int => $tag->value;

interface ExtendedUnitEnum extends \UnitEnum {}
static fn (ExtendedUnitEnum $tag): string => $tag->name;

interface ExtendedBackedEnum extends \BackedEnum {}
static fn (ExtendedBackedEnum $tag): string|int => $tag->value;
