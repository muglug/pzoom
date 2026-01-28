<?php
echo (new DateTimeImmutable("now", new DateTimeZone("UTC")))->format("Y-m-d");
