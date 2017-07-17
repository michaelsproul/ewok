#!/bin/bash

file=$1

add_line="added to our current block"
remove_line="removed from our current block"

num_add_events=$(grep "$add_line" "$file" | wc -l)
uniq_add_events=$(grep "$add_line" "$file" | sort | uniq | wc -l)
add_efficiency=$(echo "scale=4; $uniq_add_events / $num_add_events" | bc)

echo "Add events: $num_add_events, unique: $uniq_add_events, efficiency: $add_efficiency"

num_remove_events=$(grep "$remove_line" "$file" | wc -l)
uniq_remove_events=$(grep "$remove_line" "$file" | sort | uniq | wc -l)
remove_efficiency=$(echo "scale=4; $uniq_remove_events / $num_remove_events" | bc)

echo "Remove events: $num_remove_events, unique: $uniq_remove_events, efficiency: $remove_efficiency"
