
// date formatting utils - using manual formatting because Intl was giving weird results on some machines

export function formatDate(val: Date): string {
  const day = val.getDate().toString().padStart(2, '0');
  const month = (val.getMonth() + 1).toString().padStart(2, '0')
  const year = val.getFullYear();

  return day + '/' + month + '/' + year;
}

export function formatDateTime(val: Date): string {
  // date part
  const day = val.getDate().toString().padStart(2, '0');
  const month = (val.getMonth() + 1).toString().padStart(2, '0');
  const year = val.getFullYear();

  const hours = val.getHours().toString().padStart(2, '0')
  const mins = val.getMinutes().toString().padStart(2, '0');
  // const secs = val.getSeconds().toString().padStart(2, '0');

  const datePart = day + '/' + month + '/' + year
  const timePart = hours + ':' + mins;
  return datePart + ' ' + timePart;
}

export function formatEpoch(num: number): string {
  // backend sends epoch in seconds, js expects ms
  return formatDateTime(new Date(num * 1000))
}

export function formatISO(str1: string): string
{
  const d = new Date(str1);
  const res = formatDateTime(d);
  return res;
}
