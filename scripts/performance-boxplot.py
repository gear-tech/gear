import seaborn
import pandas
import json
import os
import sys

figure_width = 15.5
item_height = 0.6

current_folder = os.getcwd()
path = os.path.join(current_folder, 'results')
if not os.path.exists(path):
    os.mkdir(path)

with open(sys.argv[1], "r") as read_file:
    data_master = json.load(read_file)

with open(sys.argv[2], "r") as read_file:
    data_current = json.load(read_file)

os.chdir(path)

seaborn.set_theme(style="ticks", palette="pastel")

for pallet in data_master:
    dataframe_master = pandas.DataFrame(data_master[pallet])
    dataframe_master = pandas.melt(dataframe_master, var_name='Test', value_name='Time')
    dataframe_master['branch'] = 'master'
    
    dataframe_current = pandas.DataFrame(data_current[pallet])
    dataframe_current = pandas.melt(dataframe_current, var_name='Test', value_name='Time')
    dataframe_current['branch'] = 'current'
    
    concated = pandas.concat([dataframe_master, dataframe_current], ignore_index=True)
    
    plot = seaborn.boxplot(x='Time', y='Test',
                hue='branch', palette=["m", "g"],
                data=concated)
    
    figure = plot.get_figure()
    height = item_height * len(data_master[pallet])
    figure.set_size_inches(figure_width, height)
    figure.savefig(pallet + '.png', bbox_inches="tight")
    figure.clf()
