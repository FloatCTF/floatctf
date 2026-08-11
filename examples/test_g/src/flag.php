<?php
// FloatCTF AWDP runtime contract: platform injects FLAG env per participant.
header("Content-Type: text/plain");
echo getenv("FLAG") ?: "no-flag";
