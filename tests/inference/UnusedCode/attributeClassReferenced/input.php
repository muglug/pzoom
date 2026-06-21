<?php

#[\Attribute]
final class MyAttribute
{
}

#[MyAttribute]
function tagged(): void
{
}

tagged();
