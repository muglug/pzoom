<?php

class A
{
    public function __construct(
        /** @var "cti"|"basic"|"teams"|"" */
        public string $licenseType = "",
    ) {
    }
}

$a = new A("ladida");
$a->licenseType = "dudidu";

echo $a->licenseType;
