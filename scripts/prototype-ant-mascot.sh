#!/usr/bin/env bash

set -u

chestnut_fg='\033[38;2;177;108;70m'
chestnut_bg='\033[48;2;177;108;70m'
dark_brown_fg='\033[38;2;108;59;43m'
dark_brown_bg='\033[48;2;108;59;43m'
sand_fg='\033[38;2;207;169;126m'
sand_bg='\033[48;2;207;169;126m'
teal_fg='\033[38;2;104;158;151m'
teal_bg='\033[48;2;104;158;151m'
dim='\033[38;5;240m'
reset='\033[0m'

height=8
first_frame=true

sprite_1=(
  '..2.......2..'
  '...3.....3...'
  '....3...3....'
  '...1111111...'
  '..111111111..'
  '.114.111.411.'
  '.11..111..11.'
  '..333333333..'
  '...33...33...'
  '..33.....33..'
  '.33.......33.'
  '.............'
)

sprite_3=(
  '.2.........2.'
  '..3.......3..'
  '...3111113...'
  '..111111111..'
  '.11111111111.'
  '114.11111.411'
  '11..11111..11'
  '.33333333333.'
  '..33.....33..'
  '..33..3..33..'
  '...333.333...'
  '.............'
)

cleanup() {
  printf '%b' "${reset}\033[?25h"
}

trap cleanup EXIT
trap 'exit 130' INT TERM
printf '\033[?25l'

animated_line() {
  local variant=$1
  local row=$2
  local frame=$3

  case "$variant" in
    1) line=${sprite_1[$row]} ;;
    3) line=${sprite_3[$row]} ;;
  esac

  if ((frame == 1)); then
    case "$variant:$row" in
      1:0) line='.2.........2.' ;;
      1:1) line='..3.......3..' ;;
      1:2) line='...3.....3...' ;;
      3:0) line='..2.......2..' ;;
      3:1) line='...3.....3...' ;;
    esac
  elif ((frame == 2)); then
    case "$variant:$row" in
      1:5) line='.113.111.311.' ;;
      1:6) line='.11..111..11.' ;;
      3:5) line='113.11111.311' ;;
      3:6) line='11..11111..11' ;;
    esac
  fi
}

select_color() {
  case "$1" in
    1) fg=$chestnut_fg; bg=$chestnut_bg ;;
    2) fg=$sand_fg; bg=$sand_bg ;;
    3) fg=$dark_brown_fg; bg=$dark_brown_bg ;;
    4) fg=$teal_fg; bg=$teal_bg ;;
  esac
}

paint_pair() {
  local top=$1
  local bottom=$2
  local index
  local upper
  local lower
  local upper_fg
  local lower_bg

  for ((index = 0; index < ${#top}; index++)); do
    upper=${top:index:1}
    lower=${bottom:index:1}
    if [[ $upper == . && $lower == . ]]; then
      printf ' '
    elif [[ $upper == "$lower" ]]; then
      select_color "$upper"
      printf '%b█%b' "$fg" "$reset"
    elif [[ $upper == . ]]; then
      select_color "$lower"
      printf '%b▄%b' "$fg" "$reset"
    elif [[ $lower == . ]]; then
      select_color "$upper"
      printf '%b▀%b' "$fg" "$reset"
    else
      select_color "$upper"
      upper_fg=$fg
      select_color "$lower"
      lower_bg=$bg
      printf '%b%b▀%b' "$upper_fg" "$lower_bg" "$reset"
    fi
  done
}

render() {
  local frame=$1
  local row
  local left_top
  local left_bottom
  local right_top
  local right_bottom

  if $first_frame; then
    first_frame=false
  else
    printf '\033[%dA' "$height"
  fi

  for ((row = 0; row < 12; row += 2)); do
    animated_line 1 "$row" "$frame"
    left_top=$line
    animated_line 1 "$((row + 1))" "$frame"
    left_bottom=$line
    animated_line 3 "$row" "$frame"
    right_top=$line
    animated_line 3 "$((row + 1))" "$frame"
    right_bottom=$line

    printf '\033[2K  '
    paint_pair "$left_top" "$left_bottom"
    printf '        '
    paint_pair "$right_top" "$right_bottom"
    printf '\n'
  done

  printf '\033[2K%b       01                   03%b\n' "$dim" "$reset"
  printf '\033[2K%b  ctrl+c to stop%b\n' "$dim" "$reset"
}

frames=(0 1 0 1 0 1 0 2)

while true; do
  for frame in "${frames[@]}"; do
    render "$frame"
    sleep 0.18
  done
done
